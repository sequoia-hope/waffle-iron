# The 5 Volume-Oracle Flags, Anchored: ALL FIVE ARE KERNEL SILENT-WRONGS

**Date:** 2026-08-08 (follow-up to the oracle build, commit 4cf0f131).
**Verdict wanted:** per flagged case, "the flag is the oracle's composition bug"
vs "the kernel's output is geometrically wrong." The first sweep's base rate
warned both ways (3 of the first 4 flags were oracle-side composition bugs).

**Result: 5/5 are kernel-side.** Two catastrophic (a union that discards its
target body), three deficit-class (material lost at revolve-union steps). The
oracle survived adversarial review on every case; no oracle bug found.

## Method

Three independent measurements per case, none sharing code with the verdict
under test:

1. **In-context volume trajectory** (`examples/volume_trajectory.rs`, new):
   rebuild the document truncated after each op, tessellate ALL live bodies,
   report per-body volumes. No operand isolation involved — this is the
   kernel's own account of its own chain, so composition-semantics doubts
   about the oracle cannot explain what it shows.
2. **Integrator cross-check:** divergence-theorem volume vs the oracle's
   column-scan winding sweep on the SAME operand meshes. Two integrators,
   zero shared code. Agreement ≤ 1e-4 rel on every operand of every case.
3. **Analytic anchor (Pappus)** for the circle-revolve operands, computed from
   the `.waffle` sketch + axis parameters alone (no kernel, no oracle):
   - R0057 op1: V = θ·R_c·πr² = 2.3287e5 vs kernel operand 2.3252e5 ✓ (chord-level)
   - R0059 op1: V = 4.7995e6 vs kernel operand 4.7922e6 ✓

Composition semantics are airtight for these five: every op is `merge:true`
boss with ABSOLUTE parameters (plane from datum-anchored sketch record;
extrude depth; revolve axis_origin/axis_direction/angle in world coords), so
an isolated operand is definitionally the in-context tool. The union of the
operands is then the unique correct output set, and the oracle computes that
union exactly per column over certified operand scans.

## Per-case verdicts

### R0090 — KERNEL WRONG, −58% (base body discarded by a union)

```
after op 0: bodies=1 live total=1.7412e8   [base circle tower]
after op 1: bodies=2 live total=2.0102e8   [gear 2.6903e7, base 1.7412e8]
after op 2: bodies=2 live total=1.2375e8   [op2-tool-alone 9.6849e7, gear 2.6903e7]
```

Op2 is a `merge:true` union. Its output equals the TOOL ALONE (9.6849e7 —
byte-close to the isolated op2 solid), and the 1.7412e8 base body is gone.
`vol(A∪B) ≥ vol(A)` is not a tolerance question; the output is impossible.
Final output = op1 ∪ op2 exactly (1.23752e8 vs 1.23747e8 measured).

### R0030 — KERNEL WRONG, −30% (same mechanism, tiny scale)

Same signature at scale 1.78e-4: base 3.667e-13 discarded at op2's union;
output = op1 ∪ op2 (8.5481e-13 vs 8.5488e-13).

**RETRACTED SAME DAY — the "stacked coplanar towers" reading was an
unmeasured inference from the `EndCapPositive` selector NAME.** Measuring the
stored planes (`plane_origin`/`plane_normal` of every sketch): the prisms are
at ARBITRARY mutual angles (n·n = 0.15–0.68) with origins hundreds of units
off each other's planes — no coplanar contact anywhere in R0090/R0030 (or
R0001/F0011). The M8-family framing is dead. What both base-drop cases DO
share, measured: the operands are (near-)disjoint (composed union ≈ sum of
operand volumes to ~2e-4), op1's tool legitimately became a standalone body,
and op2's union then returned ONE output equal to the tool — where a disjoint
pair must return TWO lumps (the spec'd `split_solid_into_bodies` path,
`disjoint_merge_bodies.rs`). The sharpened mechanism suspect: the
disjoint/graze union path collapsing a 2-shell result to one shell.

### R0040 / R0057 / R0059 — KERNEL WRONG, deficit class (−2.8% / −1.3% / −1.0%)

Every deficit enters at a REVOLVE union step (R0040 op2, R0057 op1, R0059
op1). Operand solids certified by both integrators (+ Pappus where the profile
is a centered circle); the exact per-column union of those operands exceeds
the kernel's merged output. *(Initial reading "wrong face survival at curved
union seams" was REFINED by the spatial anchor below: the set/B-Rep is right;
the loss is patch-tessellation chord error.)* All three flags are invariant
to 100× finer oracle tessellation
(measured 2026-08-08), ruling out chord error.

## The three masks that let these grade SUPPORTED_CORRECT

1. **The categorized runner never checks the meta's `volume_monotonicity`
   oracle.** `assay_kv2.rs` reads euler/watertight/bbox/magnitude/
   expected_volume — the per-op direction oracle in every meta is dead data on
   this path.
2. **The v2 property pipeline downgrades I9–I12 volume-invariant failures to
   advisory PASSES** (`assay/properties_v2.rs:261-274`, "known-class issues
   that shouldn't block the test suite") — a P9/P10 violation in the harness
   itself: it is a standing decision to accommodate wrong boolean volume.
3. **The "merge incomplete" check asked a body question with a FEATURE
   counter.** `distinct_solid_count()` counts solid-bearing features (its
   limitation is documented in `combine_add_disjoint_targets.rs`: it cannot
   see a feature's `Body{index}` leftover outputs), so a union that discarded
   its target while a leftover body stayed live read as "one solid".

## The 29-case flip, and the SAME-DAY correction of my first fix

My first fix took `max(registry, live)` body count in the merge-incomplete
check. The full-corpus rerun flipped **29** cases CORRECT→WRONG — and
follow-up measurement showed the check itself was wrong for 27 of them:

- **The generator never promises connectivity.** `assay/gen.rs:760-782`
  repairs only NO-OP shapes (a swallowed boss, a free-space CUT); a
  **free-space BOSS is a sanctioned case shape** — it adds a disjoint lump
  and satisfies the meta's `volume_monotonicity: increase`.
- **The engine's two-body disjoint-merge output is spec'd, tested behavior**
  (`disjoint_merge_bodies.rs` — the "F0015-class bug" fix; F0015 was one of
  the 29). `merge:true` into a disjoint body legitimately yields two bodies.
- **The planes are at arbitrary angles** (measured: n·n = 0.15–0.68, origins
  far off each other's planes) — the "stacked coplanar towers" reading of the
  flip set was an unmeasured inference from the `EndCapPositive` selector
  name, and is retracted.
- The oracle's own sweep already certified the all-boss members of the flip
  set volumetrically (agree ⇒ composed union ≈ output): their operands are
  (near-)disjoint, so two bodies is the CORRECT geometry.

**The corrected check (shipped): volume composition, not body count.** The
categorized runner now runs the independent oracle in-line for every multi-op
case (`assay/volume_oracle_doc.rs`, extracted from the sweep test): the
output must equal the set union of the operations' isolated solids. This
distinguishes all three shapes body count conflated — legitimate disjoint
lumps (agree), material loss (R0090/R0030 base-drop), unfused overlap — and
also catches the single-body deficit class (R0040/R0057/R0059) no count could
see. Cut chains are NOT-COVERED (a cut tool is not re-authorable in
isolation) — a recorded coverage gap, never a silent pass.

## Base-drop mechanism (sharpened, one measurement short)

`feature-engine/src/rebuild.rs` CombineMode::Add (≈:1236-1345) reads the
kernel union's output count as a disjointness proof — `1 output ⇒ merged
(target lump CONSUMED, replaced by the result)`, `>1 ⇒ disjoint (keep the
original lump as a leftover under a Body{index} output)`. R0090/R0030's
operands are (near-)disjoint, so the CORRECT kernel answer at op2 is TWO
lumps (`split_solid_into_bodies`); the measured output is ONE lump equal to
the tool. Suspect: the disjoint/graze union path collapsing a 2-shell result
to one shell, which the engine then trusts, consuming the base feature.
**Next measurement:** call the kernel boolean directly on the two operand
solids and inspect the output lump count and volumes.

## The custody fix (landed 2026-08-08, same session)

`feature-engine/src/rebuild.rs`: `MostRecentLegacy` now resolves to **all live
solid outputs** of the most recent solid-bearing feature
(`find_most_recent_solid_outputs`), so whole-feature consumption only ever
hides bodies the fold took custody of (merged, or re-emitted as the consumer's
own `Body{index}` leftovers). Byte-identical whenever the target feature
carries one body — the only case the old single-handle behavior handled
correctly. The consumption walk's predicate is aligned (`Main | Body`), and
`resolve_depth`'s ThroughAll probe keeps its single-representative semantics.
Regression suite: `test-harness/tests/disjoint_merge_custody.rs` (3-op
conservation, 2-op invariance, touch-one-keep-other).

Full-corpus outcome (fast tier green first; census delta = 3, ALL
custody-explained, nothing else moved): **257C / 2W / 49E / 0T**.

- **R0030 → SUPPORTED_CORRECT.** Custody alone repairs it; composition agrees
  (rel 4.5e-5).
- **R0090 → ERROR (loud).** With custody fixed, the fold genuinely attempts
  `union(base, tool)` on what is in fact a ~0.2 %-overlap GRAZING pair — and
  that union STOPs at `ring rejected by CDT (degenerate/self-intersecting)`,
  joining the existing cdt-ring-rejected family (the §4.4.1-as-written
  epic's target class, `specs/yang_441_trim_cdt_construction.md`).
- **R0040 → ERROR (loud).** Same shape: its revolve-union deficit was also
  custody-entangled; the now-attempted union STOPs at `holed lateral CDT
  failed: CDT backend failed to triangulate` (the non-convex CDT wall).
- **R0057 / R0059 stay SUPPORTED_WRONG** — the two remaining true silent
  deficits (single-body chains; composition flags them at rel 1.3e-2 /
  1.0e-2). They are the deficit-class investigation's remaining anchors.

The base-drop cases were TWO stacked defects each: the custody deletion
(fixed) hid a genuine boolean incapability underneath (now a loud STOP in the
familiar ERROR families). The old world silently swallowed both layers.

## The deficit-class anchor (2026-08-08, follow-up session): NOT a set error — patch-tessellation chord loss

Spatial deficit map (`examples/deficit_map.rs`: per-prefix localization +
per-column missing/extra intervals with operand ownership + convergence probe
+ logical-face census) on the two remaining SUPPORTED_WRONGs:

**R0057** (rect prism + 67° circle revolve, near-disjoint pair — sum ≈
composed): the entire 3.64e3 loss is a ≤1-unit-deep SKIN over the torus
wedge's WHOLE lateral surface, uniform across its full angular span
(depth-inside-surface histogram: 95 % < 0.5, none > 1.5; φ uniform 0–67°);
gross extra ≈ 0 (pure inscribed loss, nothing displaced). **R0059**: same
signature; missing owned by the torus wedge AND the circle prism's cylinder
lateral — the curved faces.

**Convergence probe: the output mesh is FROZEN** — 794 tris (R0057) / 2605
(R0059) invariant under a 100× tessellation-tolerance sweep; the isolated
operands are also frozen but ~6× denser (4966 / 8090). **Face census: the
outputs are analytic B-Reps** (8 / 13 logical faces; the torus lateral is ONE
face) — mesh-facet emission and wrong-face-survival are REFUTED.

Code anchor, three layers:

1. `kernel_v2::adapter::tessellate` **ignores its tolerance parameter**
   (`_tolerance: f64, // planar tessellation is exact; tolerance is moot` —
   a planar-era comment whose premise died when curved geometry landed).
2. Render density is the fixed canonical band `RENDER_CHORD_TOLERANCE_REL =
   1e-3` RELATIVE sagitta (`tessellate/mod.rs:151`, N=71 per full circle).
   The structured full-surface path DELIVERS it: isolated wedge measured sag
   ≈ 0.04 ≈ 1e-3·r ✓ (its 3.4e2 Pappus gap is exactly this).
3. **Boolean-output curved faces go through the UV-CDT PATCH path**
   (`tessellate_torus_patch` etc.), whose refinement is a triangle-AREA
   budget (`max_area = seg²`, seg = meridian spacing) — but an area bound
   does not bound EDGE LENGTH, so large/skinny triangles pass with sag far
   above the band: measured output sag ≈ 0.29 ≈ **8× the canonical band**.
   Volumes measured off these meshes lose the 1.0–1.3 % the oracle flags.

**Verdict revision for the class: the solids are (plausibly) correct; the
defect is that the patch tessellation path does not meet the crate's own
documented chord band, and the whole tessellation layer is
tolerance-insensitive.** The oracle flags honestly — its band assumes
common-mode chord error and the output side breaks that by 8×; the app
displays and every mesh-based oracle measures exactly these meshes.

**Fix (next increment, recorded not built):** make the patch CDT deliver the
SAME canonical sagitta band as the structured path — bound/split edges to the
chord criterion (ℓ²/(8r) ≤ rel·r per surface principal radius), not area
alone. Acceptance: R0057/R0059 composition → agree; corpus → 259C/0W/49E/0T;
`volume_trajectory`/`deficit_map` convergence probes show the output sag at
band. (Honoring the trait tolerance parameter is a separate, deliberate
design question — crate hard rule 5 wants deterministic density — the
invariant to restore first is "same surface ⇒ same band on every path".)

## What this changes

- The **0-WRONG headline was an artifact of oracle coverage**, exactly as the
  2026-08-06 review feared: the first absolute geometric oracle over the F/R
  passing population found 5 wrongs in its first sweep (of 121 covered) — and
  all 5 are now LOUD in the canonical assay via the in-line composition
  check. Honest baseline: **256C / 5W / 47E / 0T** (5W = R0090 R0030 R0040
  R0057 R0059).
- **Priority inversion:** confirmed silent-wrongs outrank loud ERRORs (P10).
  The base-drop class (R0090/R0030) is the top kernel item, ahead of the
  ERROR-tail epic; the revolve-deficit class follows it.
- Remaining mask retirements as separate increments: meta
  `volume_monotonicity` in the categorized runner (mask 1), the properties_v2
  advisory downgrade (mask 2, fuzz path), and composition coverage for cut
  chains (oracle increment 3).

Next steps and ordering: `docs/yang_functional_roadmap.md` §0 (2026-08-08
addendum) and `specs/yang_441_trim_cdt_construction.md`.
